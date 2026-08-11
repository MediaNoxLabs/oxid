# Dioxus 0.7 dependency review

- Project: [Dioxus](https://github.com/DioxusLabs/dioxus)
- Selected version: 0.7.10 (pinned by the workspace and `Cargo.lock`)
- License: MIT OR Apache-2.0
- Maintenance/activity: active upstream monorepo with current 0.7 releases
- Security/audit evidence: no independent Oxid audit; Cargo advisory scanning is
  required. The stable desktop graph has explicit, review-dated GTK3/Wry
  exceptions in `docs/security/advisory-exceptions.md`.
- Android: supported through the Dioxus mobile renderer/WebView host
- iOS: supported through the Dioxus mobile renderer/WebView host
- Desktop: supported through the desktop renderer
- WASM/web: supported through the web renderer
- Cryptography: none selected from Dioxus; it must not handle wallet key
  operations
- API stability: pre-1.0; minor upgrades may require UI adapter changes
- Toolchain note: the current platform graph contains `block 0.1.6`, which Rust
  1.97 reports as future-incompatible. Treat an actual compiler rejection as an
  upgrade blocker and re-check this warning on every Dioxus/Wry update.
- Reason selected: Rust UI reuse across Tier-1 mobile and Tier-2 desktop/web,
  matching the product blueprint
- Alternatives considered: separate native Swift/Kotlin applications, React
  Native with Rust FFI, and a web-only shell
- Adapter boundary: `crates/ui-dioxus` and `apps/oxid`; no core crate depends on
  Dioxus
- Exit strategy: replace the incoming adapter while retaining application
  use-case traits and Oxid-owned DTOs
