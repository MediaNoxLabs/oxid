# ADR-0095: Share a closed capability manifest with the developer UI

- Status: Accepted
- Date: 2026-08-19
- Blueprint source: Sections 3–7, 12–13, 16–18, and 21
- Design source: `docs/design/ui-profiles.md` P1, P2, P7, P9, and P10; rollout Phase 4b
- Tracking: issues #2, #65, #69, and #87
- Amends: ADR-0002, ADR-0024, ADR-0080, and ADR-0085
- Implementation state: one dependency-free application crate owns the public capability manifest; headless serializes it and the opt-in standalone developer profile renders it with release exclusion proof

## Context

`system.capabilities` began as a private JSON builder inside the headless
incoming adapter. The Dioxus Diagnostics page separately hard-coded a shorter
set of capability cards. Exposing a fuller developer view by copying either
list would preserve the drift: the headless manifest already omitted required
confirmation metadata for protected key and transaction operations.

The prototype's logs, HTTP histograms, process measurements, persisted tracing
strings, and benchmark telemetry are not a suitable source. ADR-0080 explicitly
excludes those values because they can retain payloads, endpoints, identifiers,
timings, and other operational detail. The developer profile needs more public
metadata, not access to wider wallet data.

The headless and Dioxus crates are independent incoming adapters and therefore
must not depend on one another. Composition remains the only authority over
adapters and custody.

## Decision

Add `oxid-capabilities-application` as a dependency-free, UI-neutral
application boundary. It returns a typed list of method, status, and closed
public facts. Values can be stable text, booleans, closed text lists, bounded
objects, or null. Its construction context accepts only proof availability and
closed Passport Vault mode/persistence labels; unknown labels collapse to
`unavailable`. It has no field for request payloads, profile or credential
identifiers, claims, endpoints, keys, timestamps, logs, or measurements.

The headless adapter converts this owned structure to its existing JSON
contract. Dioxus consumes the same structure only behind `ui-profile-dev`.
The developer page renders raw method/status/fact strings and the truthful
manifest metadata `composition_time`, `not_applicable` cursor, and
`not_collected` timing. It does not synthesize timing samples where telemetry
is off.

The shared manifest declares every confirmation-bearing headless wallet
operation, including protected key sign/delete and transaction authorize,
submit/send, and start-submission aliases. A conformance test owns that closed
set. Existing exact intent validation in the application hexagons remains the
authorization authority; manifest metadata is descriptive and never bypasses
confirmation.

`ui-profile-dev` is an app Cargo feature forwarded to Dioxus. The app emits a
compile error unless either standalone-development or standalone-native-custody
is also selected. The feature changes only presentation after composition. A
persistent non-dismissible banner identifies the developer/standalone build.
Normal release CI builds the actual app binary and scans for the stable
developer marker; the marker must be absent.

The demo fixture drawer remains a separate Phase 4b slice. It will sequence
existing use cases without inheriting authority from this manifest.

## Consequences

- Headless and Dioxus cannot drift on which capabilities, modes, and
  confirmation gates are declared.
- The default user UI does not compile the capability crate or developer page,
  and its release binary cannot contain the developer-profile marker.
- Unknown composition labels fail closed instead of becoming free-form values
  in a debug screen or protocol response.
- The developer view remains safe to inspect but is not a readiness,
  authorization, health, or production-custody oracle.
- Adding or changing a capability now requires updating one typed manifest and
  its confirmation/public-data conformance tests.
- Building the normal release artifact becomes part of the UI repository gate.

## Validation

- Capability tests prove unique non-empty methods, the complete closed
  confirmation-required set, safe public values, and unknown-label fallback.
- Existing headless tests prove the serialized contract and native/simulated
  dynamic facts remain byte-shape compatible.
- Feature checks compile Dioxus and the app with the standalone developer
  profile and prove the same profile fails without standalone composition.
- Architecture checks keep the new application crate dependency-free and allow
  both incoming adapters to depend on it without depending on each other.
- The release check scans `target/release/oxid-app` for
  `OXID_UI_PROFILE_DEVELOPMENT` and fails if found.
- Focused iOS and Android fresh-install smokes prove the banner before
  onboarding, create a profile, open the developer page, assert the shared
  manifest source and confirmation metadata, and reject a secret request key;
  the developer page itself contains no mutable flow.

## Rejected alternatives

- Making Dioxus depend on `oxid-headless` would couple incoming adapters and
  invert the architecture.
- Duplicating a UI summary would retain the exact drift this decision fixes.
- Moving JSON values into composition would make wiring depend on an incoming
  serialization format.
- Copying prototype log/telemetry panels would violate ADR-0080 and telemetry-
  off policy.
- Reporting invented request or render timings would turn absence of telemetry
  into a misleading measurement claim.
