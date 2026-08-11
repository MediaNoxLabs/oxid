# RustCrypto Midnight HD derivation dependency review

- Projects:
  [`bip32`](https://github.com/iqlusioninc/crates/tree/main/bip32),
  [RustCrypto `k256`](https://github.com/RustCrypto/elliptic-curves/tree/master/k256),
  and [RustCrypto `sha2`](https://github.com/RustCrypto/hashes/tree/master/sha2)
- Selected versions: `bip32` 0.5.3, `k256` 0.13.4, and `sha2` 0.10.9,
  pinned exactly by the workspace and `Cargo.lock`
- Licenses: Apache-2.0 OR MIT for all three crates
- Maintenance/activity: all three source repositories were active when reviewed
  on 2026-08-12. `bip32` 0.5.3 is the latest stable release; 0.6.0-pre.1
  was rejected because a pre-release is not justified for this development
  adapter. `k256` and `sha2` stay on the compatible 0.13/0.10 generation used
  by `bip32`, avoiding a second secp256k1 implementation in this adapter.
- Security/audit evidence: no independent Oxid audit. These are hazardous
  cryptographic dependencies even though the adapter uses high-level APIs.
  Repository advisory and dependency-policy gates remain mandatory. The
  implementation is development-only and is not evidence for production
  custody.
- Android/iOS/desktop: the selected features are pure Rust. No C FFI, dynamic
  loading, assembly feature, PKCS#8, PEM, mnemonic, or key-export codec is
  enabled. Both Tier-1 target graphs remain required gates.
- WASM/web: the crates are portable, but this review does not authorize
  production browser custody or WebView key handling.
- Cryptography: BIP32 private child derivation uses
  `m/44'/2400'/account'/role/index`; the delivered role is Midnight
  `NightExternal` (`0`). The 32-byte child scalar becomes a secp256k1 BIP340
  signing key. SHA-256 of its x-only public key is encoded as the unshielded
  Midnight address payload.
- Conformance evidence: the path matches `HDWallet.ts` at `midnight-wallet`
  revision `25d0c3857fc0e20435e06a9225bd8709ecce1115`. Its lock resolves
  `@scure/bip32` 2.2.0. An independent cross-language fixture for root input
  `[0x01; 32]` and path `m/44'/2400'/0'/0/0` produces x-only public key
  `b193e54524dc796402870a883fbdcd83869c9c307dda8c0d99c5f769169fc883`
  and devnet address
  `mn_addr_devnet13gn5semyxq8w3cd9fv0av5v4crkzcfmt7mlmvh83wwu6gtc8w3sqr2gnec`.
  The official address-format JSON uses its seed as the already-derived
  unshielded scalar, so it remains a codec fixture and is not misrepresented as
  a root-to-child HD vector.
- Secret lifecycle: the root input is generated inside `storage-dev`, wrapped
  in `Zeroizing`, never accepted by the headless protocol, and never persisted.
  Derived scalar buffers and `k256` Schnorr signing keys zeroize on drop.
  `bip32` 0.5.3 does not expose whole-extended-key zeroization for every
  attribute, which is an additional reason this adapter cannot become
  production custody.
- API stability: every dependency type is contained in outgoing adapters.
  Domain, application, Dioxus, and headless DTOs contain only bounded indices,
  public addresses, public-key metadata, and opaque references.
- Reason selected: reproduce the canonical Midnight Wallet SDK derivation and
  ledger address/signature semantics without importing a TypeScript runtime or
  the ledger transaction/proving graph.
- Alternatives considered: `k256` 0.14 plus stable `bip32` (duplicates the
  curve stack), `bip32` 0.6.0-pre.1 (pre-release), `bitcoin`/`secp256k1`
  bindings (larger API and native/FFI surface), hand-written BIP32 (unacceptable
  cryptographic responsibility), and the unpublished ledger aggregate (wrong
  boundary and much larger graph).
- Adapter boundary: BIP32 and BIP340 are confined to
  `crates/adapters/storage-dev`; SHA-256 and Midnight address encoding are
  confined to `crates/adapters/midnight`.
- Deployment restriction: process-local, non-persistent,
  `development_only`; `oxid_composition::compose()` continues to use the
  fail-closed unavailable custody and account adapters.
- Exit strategy: native Apple and Android custody adapters implement the same
  derivation/key-operation ports, or a separately reviewed protected software
  root design replaces this adapter, without changing core or incoming APIs.
