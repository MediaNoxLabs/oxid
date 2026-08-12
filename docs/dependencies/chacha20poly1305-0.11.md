# chacha20poly1305 0.11 dependency review

- Project: [`RustCrypto/AEADs`](https://github.com/RustCrypto/AEADs)
- Selected version: `chacha20poly1305` 0.11.0, pinned exactly by the workspace
  and `Cargo.lock`
- License: Apache-2.0 OR MIT
- Rust requirement: 1.85; Oxid requires Rust 1.95
- Maintenance/activity: current 0.11 release from the RustCrypto symmetric
  cryptography organization; source is not archived and follows the shared
  RustCrypto release/maintenance model.
- Security/audit evidence: the crate is a pure-Rust implementation of RFC 8439
  ChaCha20-Poly1305 plus the widely deployed 192-bit-nonce XChaCha construction.
  Oxid has not commissioned an independent audit. Cargo advisory, exact lock,
  source, license, tests, and target gates remain mandatory.
- Android/iOS/desktop/WASM: pure Rust. Oxid enables the default allocation and
  OS-random support plus `zeroize`; Tier-1 native mobile builds and smoke tests
  are required. This slice does not make the browser target supported.
- Cryptography: XChaCha20-Poly1305 authenticated encryption with a 256-bit key,
  unique 192-bit random nonce, and fixed associated data. Authentication
  failure is mapped to a storage-integrity error. Keys/plaintext use zeroizing
  containers at the adapter boundary where practical.
- API stability: crate types are private to
  `crates/adapters/storage-credential-json`; core, application, composition,
  UI, and headless APIs expose no cipher, nonce, key, or ciphertext type.
- Reason selected: current RustCrypto implementation, long random-nonce margin,
  authenticated whole-document encryption, pure-Rust mobile portability, and
  reuse of the repository's reviewed RustCrypto dependency family.
- Alternatives considered: AES-GCM (safe random nonce margin is smaller),
  unauthenticated encryption (rejected), native-only storage APIs directly in
  the repository (would couple core flow tests to each OS), SQLCipher/Redb
  plaintext payloads (larger or insufficient boundary), and a custom cipher
  construction (rejected).
- Adapter boundary: standalone-development credential persistence only. The key
  file is owner-private but is not hardware-backed custody, biometric
  authorization, recovery, or backup.
- Exit strategy: keep the credential repository port and encrypted envelope
  semantics while replacing key loading/wrapping with native
  Keychain/Secure Enclave and Android Keystore adapters. A future envelope
  version can migrate algorithms without changing credential core types.
