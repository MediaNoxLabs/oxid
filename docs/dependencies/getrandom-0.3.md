# getrandom 0.3 dependency review

- Project: [getrandom](https://github.com/rust-random/getrandom)
- Selected version: 0.3.4 (pinned by the workspace and `Cargo.lock`)
- License: MIT OR Apache-2.0
- Maintenance/activity: maintained by the Rust Random project
- Security/audit evidence: no independent Oxid audit; advisory scanning is
  mandatory
- Android/iOS/desktop: uses supported operating-system entropy sources
- WASM/web: explicit `wasm_js` backend selected for the web adapter boundary
- Cryptography: supplies operating-system random bytes; it does not implement
  wallet key algorithms
- API stability: stable small API within the 0.3 line
- Reason selected: direct OS randomness with a narrow implementation surface
- Alternatives considered: `rand` facade and platform-specific FFI
- Adapter boundary: `crates/adapters/platform-system`; core code sees only
  `RandomPort`
- Exit strategy: replace the adapter without changing application/domain types
