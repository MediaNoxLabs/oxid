# argon2 0.5 dependency review

- Project: [`RustCrypto/password-hashes`](https://github.com/RustCrypto/password-hashes/tree/master/argon2)
- Selected version: `argon2` 0.5.3, pinned exactly by the workspace and
  `Cargo.lock`
- License: Apache-2.0 OR MIT
- Rust requirement: 1.65; Oxid requires Rust 1.95
- Maintenance/activity: stable RustCrypto release; the repository is active and
  not archived. Pre-release 0.6 versions are intentionally not selected.
- Security/audit evidence: pure-Rust implementation of Argon2d, Argon2i, and
  Argon2id. Oxid selects Argon2id v1.3. Oxid has not commissioned an independent
  audit; exact locking, RustSec, source, license, tests, and target gates remain
  mandatory.
- Android/iOS/desktop/WASM: pure Rust. Oxid enables only `alloc` and `zeroize`,
  not the crate's password-hash string or RNG convenience surface. Tier-1 mobile
  builds and resource measurements remain required before release.
- Cryptography: `hash_password_into` derives exactly 32 bytes from the bounded
  recovery secret and random 16-byte salt. Parameters are fixed at 19,456 KiB,
  two iterations, and one lane. Untrusted package fields cannot select weaker or
  more expensive values.
- API stability: Argon2 types remain private to
  `crates/adapters/backup-portable`; wallet domain, application, UI, native
  bridge, and headless APIs expose none of them.
- Reason selected: reviewed password KDF, pure-Rust target portability, explicit
  parameter construction, no runtime service, and compatibility with the
  repository's existing RustCrypto/zeroization policy.
- Alternatives considered: PBKDF2 (less memory-hard), scrypt (viable but not
  selected), platform-only KDF APIs (would fragment the portable format), the
  live vault unlock credential/ciphertext (not portable), and a custom KDF
  (rejected).
- Adapter boundary: explicit portable custody package only. It is not used for
  device vault encryption, ordinary wallet unlock, credential storage, or
  network authentication.
- Exit strategy: retain the versioned application port and envelope. A future
  format version may select new reviewed parameters or a different KDF while
  version 1 stays strict and never accepts a silent downgrade.
