# url 2.5 dependency review

- Project: [servo/rust-url](https://github.com/servo/rust-url)
- Selected version: 2.5.8, exactly pinned in the workspace and `Cargo.lock`
- License: MIT OR Apache-2.0
- Maintenance/activity: established Rust URL implementation; upgrades remain
  explicit lockfile reviews
- Security/audit evidence: no independent Oxid audit; repository advisory,
  source, license, and test gates inspect every resolved update
- Android, iOS, desktop, and WASM/web: portable Rust parsing; this slice uses no
  platform networking API
- Cryptography: none
- API stability: stable WHATWG URL parsing and percent-encoded query-pair APIs
- Reason selected: parse the custom `openid-credential-offer` URI, decode its
  embedded offer parameter, and validate endpoint scheme/host/userinfo/fragment
  structure without handwritten URI parsing
- Alternatives considered: `http::Uri`, manual splitting, and a full OIDC SDK.
  `http::Uri` does not model custom-scheme query handling, manual parsing is
  security-sensitive, and a full SDK is disproportionate for the reviewed
  standalone subset.
- Adapter boundary: direct dependency of `crates/adapters/openid4vci` only;
  protocol domain/application and incoming adapters remain URL-type-free
- Exit strategy: replace URI parsing inside the OID4VCI adapter while
  preserving the owned protocol ports and application commands
