# directories 6 dependency review

- Project: [directories-rs](https://github.com/dirs-dev/directories-rs)
- Selected version: 6.0.0, exactly pinned in the workspace and `Cargo.lock`
- License: MIT OR Apache-2.0
- Maintenance/activity: the GitHub repository is archived and directs ongoing
  development to Codeberg; upgrades therefore require an explicit source and
  maintenance re-check
- Security/audit evidence: no independent Oxid audit; `cargo audit` plus source,
  license, and advisory gates run for every repository change
- Android: not used for the default path because native Android application
  processes do not reliably expose a home directory; the adapter uses the
  application `Context` instead
- iOS: supported by the crate's Apple implementation and resolves beneath the
  application container
- Desktop: resolves conventional local application data directories on Linux,
  macOS, and Windows
- WASM/web: returns no project directory; durable web storage will require a
  separate adapter
- Cryptography: none; this dependency must never select or implement secret
  storage
- API stability: stable small API at version 6; the exact pin and adapter
  isolation contain upstream changes
- Reason selected: avoids handwritten platform path rules for the public JSON
  profile metadata adapter
- Alternatives considered: environment-only paths, application-relative files,
  and platform-specific path code. They are less suitable for a standalone
  mobile-first application and would duplicate OS conventions.
- Adapter boundary: direct dependency of `crates/adapters/storage-json` only;
  core domain, application, and platform-port crates remain independent
- Exit strategy: replace directory selection inside the JSON adapter or replace
  the adapter entirely while retaining `WalletProfileRepository`
