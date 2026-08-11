# RustSec advisory exceptions

- Decision date: 2026-08-11
- Next mandatory review: before 2026-10-01, every Dioxus/Wry upgrade, and before
  any release described as production-capable
- Scope: M0 profile shell only; no asset keys, seeds, DIDs, credentials, or
  durable storage

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

## Enforcement

- New advisory IDs fail CI.
- The quality gate runs Cargo audit with warnings denied and only these explicit
  exceptions.
- Cargo deny independently enforces license, source, wildcard, and duplicate
  dependency policy.
- Production custody work cannot proceed while an applicable unsoundness
  exception remains unreviewed.
