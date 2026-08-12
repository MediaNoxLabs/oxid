# RustCrypto development signing dependency review

- Projects:
  [ed25519-dalek](https://github.com/dalek-cryptography/curve25519-dalek),
  [RustCrypto P-256](https://github.com/RustCrypto/elliptic-curves), and
  [zeroize](https://github.com/RustCrypto/utils/tree/master/zeroize)
- Selected versions: `ed25519-dalek` 3.0.0, `p256` 0.14.0, and `zeroize`
  1.9.0, pinned by the workspace and `Cargo.lock`
- Licenses: BSD-3-Clause for `ed25519-dalek`; Apache-2.0 OR MIT for `p256`
  and `zeroize`
- Maintenance/activity: all three upstream repositories were active when
  reviewed on 2026-08-11; the selected Ed25519 and P-256 releases are current
  major lines
- Security/audit evidence: no independent Oxid audit. The Dalek project has
  prior external audit history, and the selected 3.x API is newer than the
  public-key/signing-oracle issue fixed in 2.0. RustCrypto and Dalek security
  policies plus Oxid advisory scanning remain mandatory. A new-major software
  stack is not evidence of production custody.
- Android/iOS/desktop: pure Rust libraries compile without an OS key service;
  mobile target compilation is a required gate
- WASM/web: no production web custody is authorized by this review
- Cryptography: deterministic Ed25519 signatures and RFC 6979 P-256 ECDSA
  signatures, plus public-key verification of DID-bound OpenID4VCI proof JWTs;
  only high-level signing and verification APIs are enabled. Hazardous, private
  key import, PKCS#8, PEM, batch, ECDH, and legacy-compatibility features are
  not enabled.
- Secret lifecycle: signing-key implementations provide zeroization on drop;
  transient random scalar/seed buffers are wrapped in `Zeroizing`
- API stability: both crypto crates are new major versions; the adapter owns
  every external type so replacement does not affect core APIs
- Reason selected: exercise real, independently verifiable opaque-reference
  signing flows in the standalone harness without copying prototype crypto or
  raw-key DTOs
- Alternatives considered: fake signatures (insufficient conformance value),
  ring (different algorithm/API coverage), Askar (broader encrypted-store/KMS
  responsibility), and native platform APIs (required for production but not
  available in a host-independent headless test)
- Adapter boundary: `crates/adapters/storage-dev` owns development signing and
  `crates/adapters/openid4vci` owns issuer-side public proof verification;
  domain/application crates do not depend on cryptography libraries
- Deployment restriction: process-local, non-persistent,
  `development_only`; `oxid_composition::compose()` uses the fail-closed
  unavailable adapter instead
- Exit strategy: remove or replace this adapter without changing wallet domain,
  application, Dioxus, headless protocol, or native platform adapters

Midnight BIP32/secp256k1-Schnorr derivation is reviewed separately in
[rustcrypto-midnight-hd-derivation.md](rustcrypto-midnight-hd-derivation.md).
