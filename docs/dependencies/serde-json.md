# Serde and serde_json dependency review

- Projects: [Serde](https://github.com/serde-rs/serde) and
  [serde_json](https://github.com/serde-rs/json)
- Selected versions: Serde 1.0.229 and serde_json 1.0.151, exactly pinned in the
  workspace and `Cargo.lock`
- License: MIT OR Apache-2.0
- Maintenance/activity: established 1.x projects; version changes remain
  explicit lockfile reviews
- Security/audit evidence: no independent Oxid audit; `cargo audit` and source,
  license, and advisory gates run for every repository change
- Android, iOS, desktop, and WASM/web: platform-independent serialization
  libraries; native profile metadata and the headless transport both use JSON
- Cryptography: none; neither dependency may handle or define key custody
- API stability: stable 1.x serialization traits and JSON data model, with the
  headless wire format additionally pinned by ADR-0024
- Reason selected: strict typed decoding and deterministic JSON encoding for
  the versioned NDJSON headless adapter and versioned public profile document
- Alternatives considered: handwritten JSON parsing, `simd-json`, and binary
  formats such as CBOR/Postcard. Hand parsing is riskier; SIMD is unnecessary at
  this scale; binary formats make shell and test interoperability worse.
- Adapter boundary: direct dependencies of `apps/oxid-headless` and
  `crates/adapters/storage-json` only. Oxid's foundation, domain, application,
  and port crates remain dependency-free from serialization frameworks.
- Exit strategy: replace serialization inside the headless adapter while
  preserving application commands and the versioned protocol contract
