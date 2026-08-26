# ADR-0103: Regrow oversized adapters behind capability façades

- Status: Accepted
- Date: 2026-08-26
- Blueprint source: Sections 3, 6, 13, 14, 18, and 19
- Source: [issue #145](https://github.com/MediaNoxLabs/oxid/issues/145)
- Amends: ADR-0001, ADR-0002, ADR-0020, and ADR-0024
- Implementation state: the policy and baseline are recorded; no Rust source or crate extraction is part of this decision

## Context

Oxid's vertical delivery preserved its dependency direction, but repeated slices
accumulated in four crate roots. At `integration@21ec123`, the checked-in Rust
source measures as follows:

| Target | Crate-root lines | Crate `src/**/*.rs` lines |
| --- | ---: | ---: |
| `apps/oxid-headless` | 9,020 | 9,043 |
| `crates/ui-dioxus` | 13,489 | 14,437 |
| `crates/composition` | 4,514 | 6,758 |
| `crates/adapters/midnight` | 2,340 | 21,677 |

The crate totals include colocated tests and exclude generated artifacts and
fixtures. Across the 45 workspace packages with Rust source, the same measure
has a median of 978 lines. These measurements identify a source-cohesion
problem; they do not show a missing domain boundary or justify four rewrites.

The first delivery targets are the headless and Dioxus incoming adapters.
Composition remains third and the Midnight adapter fourth because changing
wiring or ledger-adapter ownership before the incoming façades are stable would
make behavior-preserving review harder. This ordering does not make Dioxus the
application architecture and does not move composition authority into either
incoming adapter.

## Decision

### Keep crate roots as stable capability façades

Regrow each oversized root into private, capability-cohesive source modules
inside its existing crate. A capability module owns its incoming DTO or UI
state, handler or component, boundary mapping, capability-specific helpers, and
tests. Splits follow product capabilities such as wallet profiles, accounts and
transactions, identity, credentials and protocols, diagnostics, and Passport
Vault. They do not create horizontal `handlers`, `models`, `errors`, `common`,
or `utils` dumping grounds.

The crate root remains the façade. It owns module declarations, genuinely
cross-capability dispatch or shell wiring, and the existing public surface. New
capability behavior starts in its capability module after that module exists;
it does not regrow in `lib.rs`. Each decomposition delivery records the root
line count after its last slice. Later root growth is limited to declarations,
re-exports, and cross-capability wiring and must not raise that baseline overall
without a separately reviewed cohesion reason.

The stable public paths are unchanged:

- `oxid_headless::{PROTOCOL_VERSION, HeadlessWallet, HeadlessIoError}`;
- `oxid_ui_dioxus::{App, WalletUiServices}` and every existing root-level
  `*UiServices` type;
- `oxid_ui_dioxus::{BrandProfile, SecurityCopySnapshot,
  security_copy_snapshot}`;
- `oxid_ui_dioxus::CapabilityManifestContext` only with `ui-profile-dev`.

Moved public items are re-exported at those crate-root paths. Capability source
modules remain private in the first pass, so callers cannot acquire a second,
accidental public path. Constructor signatures, trait-object bounds, error
traits, and visibility do not change.

### Preserve bytes, behavior, configuration, and tests

Headless moves preserve the exact NDJSON contract for every fixed request
stream: protocol and method strings, aliases, validation order, JSON field
names and values, error codes and messages, serialized response bytes, one
newline and flush per response, shutdown behavior, and the rule that stdout
contains protocol bytes only. A move is not an opportunity to normalize JSON,
rename a method, reorder validation, widen a bound, or change diagnostic
publication.

Dioxus moves preserve checked-in CSS, SVG/QR output, build markers, labels and
copy bytes. They also preserve the rendered routes, accessibility semantics,
component properties, service construction and getters, event-to-intent
ordering, screen-privacy behavior, secret masking, and native worker boundary.
Incidental framework implementation details are not a new public contract, but
the user-visible desktop behavior and existing test observations are.

All existing `cfg` and feature boundaries move with the item they guard. In
particular, `target_arch = "wasm32"`, `ui-profile-dev`, `ui-profile-demo`,
`app-profile-authority`, and `mobile` retain their current meanings and Cargo
forwarding. No decomposition slice adds a default feature, makes an optional
dependency unconditional, or uses a feature to select behavior that is
currently runtime-selected. Headless remains a native incoming adapter; this
policy does not add a mobile headless surface.

Tests move in the same slice as the capability they protect. Capability-local
fixtures and assertions become a sibling `tests` module or a private
capability test module; cross-capability wire and shell tests remain at the
façade. Existing tests are not replaced with compile-only checks. Every slice
adds or retains a crate-root public-path assertion, exact headless wire fixture
or focused desktop UI observation as applicable, and passes the unchanged
architecture and coverage gates.

### Use source modules before crates

No crate is extracted in this decision or in the first decomposition pass. A
capability must first exist as a cohesive source module and demonstrate a real
independent dependency, target, feature, or ownership boundary. Convenience,
file length alone, or parallel ownership is insufficient.

A later crate proposal must measure the candidate subtree and the workspace
again. The candidate must contain at least the greater of 1,000 physical Rust
source lines or the then-current workspace-package median, including its
colocated tests and excluding generated sources and fixtures. At this baseline
the threshold is 1,000 lines, rounded up from the measured 978-line median. It
must also remove or independently gate at least one dependency or target
boundary in the parent; crossing the line threshold does not by itself require
or authorize extraction. A later ADR must approve the new dependency edge and
update the architecture allowlist. These conditions prevent micro-crates while
leaving a reversible path when a boundary becomes real.

### Deliver reversible slices in fixed order

Each slice moves one coherent capability, updates its tests, and leaves Cargo
dependencies and behavior unchanged. Pure moves are separated from later
cleanup. The root continues to compile and expose the same paths after every
slice, so any slice can be reverted without reverting another capability.

Delivery order is:

1. `apps/oxid-headless/src/lib.rs`;
2. `crates/ui-dioxus/src/lib.rs`, with desktop-focused evidence;
3. `crates/composition/src/lib.rs` in a later delivery;
4. `crates/adapters/midnight/src/lib.rs` in a later delivery.

Android, iOS, simulator, emulator, physical-device, and private-credential work
is explicitly deferred. Shared Dioxus `cfg` boundaries must remain intact, but
this decision creates no mobile evidence or claim. It also does not authorize
protocol, custody, credential, ledger, composition, or UI behavior changes.

## Consequences

- Review can follow one capability from incoming boundary through mapping and
  tests without changing crate ownership or the architecture graph.
- Existing downstream imports and feature-selected builds retain one stable
  façade while internal files can be moved incrementally.
- Root regrowth has a measurable post-decomposition baseline, and future crate
  extraction has both a size floor and an architectural-benefit requirement.
- The first work can be validated with headless and desktop-focused checks;
  mobile and sensitive-data gates remain neither weakened nor claimed.
- Composition and Midnight cohesion remain visible debt with an explicit later
  order rather than being mixed into the incoming-adapter work.

## Validation for decomposition deliveries

- Record before/after root and crate source-line measurements.
- Run formatting, the unchanged architecture checker, and the affected crate's
  existing tests.
- For headless slices, compare representative success, rejection, alias, and
  shutdown response streams byte-for-byte.
- For Dioxus slices, run focused desktop tests for moved state/components and a
  desktop compile or smoke check under every affected UI profile.
- Run the repository's ordinary strict gate before integration; do not reduce
  coverage, architecture, lint, security, DCO, or signature requirements.

## Rejected alternatives

- **Extract a crate per capability now.** File size is not a dependency
  boundary, and immediate extraction would create micro-crates and architecture
  churn before cohesion is demonstrated.
- **Rewrite all four roots together.** A big-bang move obscures regressions,
  couples unrelated reviewers, and is not independently reversible.
- **Split by technical layer.** Central `handlers`, `models`, `errors`, or
  `utils` modules preserve the same cross-capability coupling under new names.
- **Publish capability submodules.** New public module paths create an avoidable
  compatibility surface; crate-root re-exports preserve the existing one.
- **Generate dispatch or introduce a façade framework.** Macros, registries, or
  a new dependency would add behavior risk to a source-organization change.
- **Clean up behavior while moving it.** Renames, new abstractions, validation
  changes, and UI redesign prevent byte-for-byte or observation-for-observation
  review and belong in later issue-backed slices.
- **Start with composition, Midnight, or mobile evidence.** That reverses the
  approved ordering and widens this policy-only delivery into wiring, ledger,
  platform, or private-credential work.
