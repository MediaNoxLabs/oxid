# RustSec advisory exceptions

- Initial decision date: 2026-08-11
- Last updated: 2026-08-12
- Next mandatory review: before 2026-10-01, every Dioxus/Wry upgrade, and before
  any release described as production-capable; the Midnight exception is also
  reviewed on every Midnight dependency update
- Scope: development-stage Dioxus shell and native Midnight adapter. The
  transaction path can balance DUST, prove through an explicitly configured
  proof server, and submit in development/headless mode, but cannot claim
  production custody or private local mobile proving.

`scripts/check-advisories.sh` denies all Cargo audit warnings except the exact
IDs below. These are transitive dependencies of Dioxus 0.7.10 and its desktop
WebView stack; Oxid does not depend on them directly.

## GTK3 maintenance advisories

Accepted temporarily:

- `RUSTSEC-2024-0411`
- `RUSTSEC-2024-0412`
- `RUSTSEC-2024-0413`
- `RUSTSEC-2024-0414`
- `RUSTSEC-2024-0415`
- `RUSTSEC-2024-0416`
- `RUSTSEC-2024-0418`
- `RUSTSEC-2024-0419`
- `RUSTSEC-2024-0420`

These mark the Dioxus/Wry Linux GTK3 bindings as unmaintained and provide no
safe compatible upgrade. They are target-specific and do not enter the Oxid
domain/application crates. Remove them together when stable Dioxus/Wry moves to
a maintained Linux WebView binding. Do not expand this set for a new UI stack
without a new review.

## Constrained unsoundness advisories

- `RUSTSEC-2024-0429` affects `glib::VariantStrIter` below version 0.20. Oxid
  does not call this API; `glib 0.18.5` is present only in the Linux GTK3 path.
  The M0 UI handles public profile labels and no secrets. Remove this exception
  as soon as the Dioxus/Wry graph uses `glib >=0.20`.
- `RUSTSEC-2026-0097` affects `rand 0.7.3` only when a custom logger recursively
  calls thread RNG while it reseeds. Here it is a build dependency of
  `phf_codegen -> selectors -> kuchikiki -> Wry`; Oxid neither configures that
  build dependency nor calls it at runtime. Remove it when Wry's parser graph
  no longer resolves `rand 0.7`.

## Other unmaintained transitive crates

- `RUSTSEC-2024-0370` (`proc-macro-error`) is used by GTK3 proc macros.
- `RUSTSEC-2024-0436` (`paste`) is used by Dioxus desktop's image-codec graph.
- `RUSTSEC-2025-0057` (`fxhash`) is used by Wry's HTML parser.

Each has no compatible safe upgrade at the Oxid boundary. They carry maintenance
risk but no vulnerability described by the advisory. Remove each exception when
its owning upstream graph drops the crate.

## Midnight ledger maintenance advisory

- `RUSTSEC-2025-0141` classifies `bincode 2.0.1` as unmaintained and reports no
  vulnerability or patched version. The immutable Midnight ledger revision
  `d9414884db9da9e9b1f6f3a7f742d79a5732f817` pulls it through
  `midnight-zk-stdlib -> midnight-transient-crypto`.
- Oxid does not call `bincode` directly. Replacing a serialization dependency
  inside the consensus/proof dependency graph locally would create a larger
  compatibility risk than this development-only exception.
- Issue #10 tracks moving to an upstream Midnight revision or compatible graph
  that removes `bincode`. This exception blocks production custody/release until
  it is re-reviewed, and must be removed as soon as the selected upstream graph
  no longer resolves the crate.

## Subxt submission-graph advisories

- `RUSTSEC-2026-0173` classifies `proc-macro-error2 2.0.1` as unmaintained.
  It is an active build-time dependency of `subxt-macro 0.44.3`; it is not
  linked as runtime wallet logic. The reviewed Subxt submission surface matches
  the prototype, and the latest Subxt release checked on 2026-08-12 still used
  the same macro dependency. Remove the exception when Subxt migrates, or when
  Oxid replaces the aggregate Subxt client behind the submission adapter.
- `RUSTSEC-2025-0161` classifies `libsecp256k1 0.7.2` as unmaintained.
  `RUSTSEC-2026-0002` and `RUSTSEC-2026-0253` describe soundness defects in
  `lru 0.12.5`. These packages are present only in Cargo's resolved optional
  `subxt-lightclient -> smoldot` lockfile graph. Oxid does not enable Subxt's
  light-client feature, and `cargo tree` confirms neither package is in the
  enabled native or WebAssembly trees. Cargo audit scans every lockfile package,
  including disabled optionals, so the exact IDs are ignored to preserve a
  deny-by-default gate without claiming they execute.
- Enabling Subxt light-client support is forbidden under these exceptions. It
  requires an ADR/dependency review and removal or replacement of the affected
  packages first.

## Enforcement

- New advisory IDs fail CI.
- The quality gate runs Cargo audit with warnings denied and only these explicit
  exceptions.
- Cargo deny independently enforces license, source, wildcard, and duplicate
  dependency policy.
- Production custody work cannot proceed while an applicable unsoundness
  exception remains unreviewed.
